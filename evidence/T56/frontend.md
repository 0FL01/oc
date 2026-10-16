# T56 — native Terminals composer and cell pane (partial qualification)

Date: 2026-10-06. Base: pushed `5b165af61`; follows PTY foundation `88e6631d0`.
R1–R3 frontend implementation is present and owning tests pass. This receipt is
**not** complete TERM01 or T44/VIS39 paired visual qualification. Actual rebuilt
debug/release PTY, process/crash/resource campaign is the next slice.

## Owners and observable behavior

- `oc-tui/src/terminal_view.rs` owns only disposable list/pane/focus state. Linux
  capability comes from the application; disabled platforms omit the registered
  terminal controls. There is no config flag, startup shell, model-visible tool,
  descriptor/emulator in the UI or new preferences store.
- Existing keymap/configuration projects `<leader>left/right/down/t/up`, comma
  leader alternatives and disabled bindings. `/terminal` explicitly creates;
  selection opens the **lower Terminals composer**, not a generic modal. Inventory
  labels use actual foreground process/title; Up/k and Down/j wrap through entries
  and `+ New terminal`. With no visible terminal the initial cursor is undefined
  and immediate Enter is inert. Matching row mouse-up activates after close.
- Toggle queries fresh inventory and chooses its **last** entry, or uses a captured
  creation request when empty. Hide/close/toggle-off acknowledges selected-ID
  clearing without process termination. Draft/chips/caret stay in their original
  TuiState. The first session-pane press from terminal focus consumes its matching
  release; transcript wheel preserves terminal focus. Pane wheel changes bounded
  owner scrollback and input restores the live bottom.
- Binary raw key/paste interception precedes editor/global/permission dispatch.
  Only the configured leader or its active sequence bypasses it; arbitrary
  non-leader pane-control remaps do not. Raw Ctrl+C/D, UTF-8, application cursor
  keys, modifier sequences and bracketed paste reach the acknowledged real owner.
  Input is refused before screen attachment is ready, never sent to an unbound
  terminal. Normal binary query/attach is serialized before event dispatch.
- Snapshot establishes the old output cursor; replay returns an atomic bounded
  owner-emulated screen **at its next cursor**, with exact byte tail or explicit
  ring-gap reset. No second VT parser or raw escape write to the host exists.
  Coalesced revision notifications (at most once per 33ms per PTY) update active
  and parked source views; hidden drains continue independently. Lifecycle clears
  missing selection/focus rather than preserving an invisible input sink.
- Ratatui paints cells, Unicode/wide continuations, supported attributes and actual
  cursor. Default/ANSI16 use donor terminal semantic theme roles, not grammar
  colors; dark/light projection is tested. Resize uses the actual session/right
  split, horizontal/vertical tabs and devtools geometry, updating the same native
  PTY/emulator identity. This is functional geometry, not paired VIS39 PASS.
- Composer and session-frame create/load errors retain `Unable to load terminal`
  from their pinned source callsites. Other stream/control failures use the actual
  safe typed error, not an invented universal toast or capability disablement.

## Captured child source correction

Inspection showed that current-root-only creation could reject a live linked child
after its parent changed Location, or choose the newer root shell/environment.
The fix uses the existing retained child runtime ownership lineage, not a guessed
path, recreated runtime or new registry:

- `runtime/terminals.rs` validates actor and immutable parent/child/operation/
  Location/generation/delivery/agent/model receipt against `runtime/children.rs`.
  A retained child creates through its original admitted shell/environment/source;
  current-root creation still fences the current worker epoch. `TuiChrome` carries
  a separate terminal epoch rather than borrowing a child's model generation.
- Native spawn uses the admitted `Shell` and its no-follow pinned cwd, not a fresh
  pathname-canonicalized authority. Credential-free `child_env` remains shared.
- Without a retained lane, a **settled** child can explicitly create only in the
  same currently admitted Location with the captured current epoch and stored
  receipt. A missing running lane/foreign source refuses; no child restart or
  command replay occurs. Existing PTYs retain their original context regardless.
- Composer activation captures the full child receipt, closes first (returning to
  the parent), then dispatches to the original child actor. Results update the
  parked child view, never the newly focused parent terminal selection.

## Executed checks and material experiments

With approved TMPDIR, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, normal stacks and
serial Cargo runs:

| Check | Actual result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib term01_` | 8 passed / 0 failed. Real PTYs, owner/application fences and retained-source fixture. |
| `cargo test --locked -p oc-tui --lib` | 444 passed / 0 failed, including 3 new Terminals scenarios and existing input/layout/theme/goldens. |
| `cargo test --locked -p oc --bin oc` | 91 passed / 0 failed, including real controller close-before-dispatch child test. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit 0 after final source admission change. |
| fmt, progress structure, diff whitespace | Exit 0. |

The retained-source fixture uses an admitted runtime/child receipt fixture and
**real OS PTYs**, not a real model-driven child launch. It creates Bash in A after
the parent context becomes Sh/B, rejects eight altered identity fields and wrong
actor, verifies source shell/cwd/generation and synthetic-key exclusion, refuses
missing live source, allows explicit settled same-Location current Sh, and reaps
both PTYs. Actual-binary child-route proof still belongs to TERM01 below.

Initial failures were diagnostic: real-consumer private viewport/selection seams
were exposed narrowly; tests stopped assuming nonexistent getters/PromptRow Eq;
default devtools geometry was corrected in the fixture; new replay screen was boxed
instead of suppressing Clippy's enum-size warning. Filtering terminal slash actions
after alignment initially broke the existing full-inventory padding test; production
now filters before measuring, and that unchanged test passes. A broad two-crate
test command timed out during cumulative compile/integration work and is **not** a
claimed full-suite PASS; isolated current gates above completed without timeout
increase. The first child-source compile used a private CoreError import; it now
uses the existing public session type. No failing tests/baselines were disabled.

## Remaining T56 qualification

Actual normal debug/release binary TERM01 is NOT_RUN: two terminals, early/raw
input, resize/VT/flood/delayed attach, hide-last/reopen/draft/focus, actual child
close/target, exit/remove/shutdown/crash/restart, stale identity and independent
process/descriptor/resource facts. The native termination helper's ignored signal/
proc-enumeration failures remain an R4 concern for that slice, not a successful
cleanup claim. Final full workspace/report/finish follow only after all R1–R4 are
verified. T44 remains PAUSED; T57 and user-owned `.opencode/` remain untouched.
