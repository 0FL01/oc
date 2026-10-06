# Goal: Native session-local interactive terminals

Status: active
Contract status freezes the approved finish line; execution status is `active` in
`progress/STATE.json`. T53 is complete; T56 is next chronologically ready. T44 remains PAUSED.
Source: owner request for full OC2 child-TUI/Subagents/Shell/Terminals parity,
explicit choice «Включить Terminals» and instruction to record the detailed plan
and commit/push, 2026-10-01. Donor OC2 v2.0.12 at
`2670273ff17da96f85c5826ced57aa1b368754fa`.
Last updated: 2026-10-06

## Objective

The native `oc` has real session-associated interactive Linux PTYs, selected or
created from Terminals and displayed in the right terminal pane. They remain alive
while hidden or while the view changes within the application. They are distinct
from T50 one-shot shell jobs. Real input/output/resize/focus and owned cleanup are
qualified by TERM01; exact presentation is separately qualified by T44/VIS39.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: One native owner creates and supervises real session-local PTYs.
  - Source: approved Terminals inclusion; donor U80/U81/U84/U85/U88.
  - Acceptance: typed local application commands/queries/events cover bounded
    list/get/create, input, resize, snapshot/output cursor and owned exit/remove.
    IDs, source session, original admitted Location/config generation, selected
    shell/cwd and lifecycle are actual owner facts, never transcript inference.
    Creation is an explicit user action, not a model tool or startup side effect.
    Linux resolved `session.terminal` is enabled and does not auto-spawn a PTY;
    donor U88 derives it from platform, not a user-authored option. Do not invent a
    compatibility config flag. Platform-disabled capability omits terminal controls;
    Linux create/refresh failure keeps controls and the source callsite's error
    feedback (composer: `Unable to load terminal`), not a silent capability
    disablement/success stub. Do not collapse all callers to one invented toast.
  - Acceptance: use the admitted configured interactive shell and session directory,
    preserve native trust/path/source and sanitized credential-free shell env
    boundaries. Parent move does not migrate a running child/PTY or retarget its
    controls; an existing PTY keeps its original execution context. Validate actor,
    target identity and generation before creation/control. No shell/ShellNotice
    job is masqueraded as a persistent terminal.
  - Primary evidence: TERM01 real PTY fixture verifies shell/cwd, IDs/ownership,
    inventory, genuine byte exchange and exit/reap; existing shell security and
    A02/A10/A13 regressions are reused, not reimplemented.
  - Status: in_progress
  - Evidence: evidence/T56/owner.md, application.md and frontend.md — real native owner, acknowledged source/epoch controls and retained-child creation; full actual-binary TERM01 pending.

- R2: The real frontend selects, creates, hides/shows and focuses terminals.
  - Source: donor Terminals/session-terminal/session-frame/keymap U80–U83/U88.
  - Acceptance: Terminals lists existing session PTYs plus `+ New terminal`, displays
    actual foreground-process/title fallback and current/focused selection. Enter
    or row mouse-up selects/creates; Up/k and Down/j wrap, unlike the Subagents
    first-Up-close rule. Without a visible terminal the initial selection is
    undefined and immediate Enter is a no-op until a row is selected. Activation
    closes the lower composer first (a child close returns to its parent), then
    dispatches select/create through an explicit admitted session/PTY binding.
    Qualify the actual reference route/target result, not an assumed child-local
    modal or pane. Removed selection clears; selection survives remount/session
    view switches, except the source's explicit hide/close clearing below. Neither
    transition kills or duplicates the process.
  - Acceptance: `/terminal` creates a new terminal; toggle-on refreshes and chooses
    the **last terminal in inventory**, or creates one if inventory is empty.
    Select opens the lower Terminals composer (including
    `+ New terminal`), not a generic modal picker. Hide/toggle-off/close clears the
    persisted selected ID and restores session focus, **without killing the PTY**.
    Effective keybindings default to `<leader>left/right` pane
    focus, `<leader>down` select, `<leader>t` toggle and `<leader>up` close. Native
    configuration uses existing generation/keymap owners, no new preferences store.
  - Acceptance: terminal focus owns ordinary raw keys, including Ctrl+C/D; the
    interceptor bypasses only the configured leader key or an active leader
    sequence. Arbitrary non-leader pane-control remappings are not automatically
    exempt from raw-key priority. Raw Ctrl+C/D reach the PTY instead of global quit,
    composer interrupt or input deletion. Returning to session restores its draft,
    chips/cursor/focus; first click from the terminal only focuses the session and
    consumes the matching release before transcript/permission actions. Wheel over
    transcript scrolls without stealing terminal focus. Exit/disconnect restores
    usable session focus and honest inventory, not a stuck invisible input sink.
  - Primary evidence: TERM01 rebuilt actual-binary PTY sends known input and raw
    control bytes, observes child output, switches/hides/reopens, and independently
    checks process identity, no duplicate creation and focus/draft preservation.
    T44 reuses the same fixture for paired visual qualification, not a reverse gate.
  - Status: in_progress
  - Evidence: evidence/T56/frontend.md — lower composer, captured child close/target, raw/leader/focus routing and acknowledged real-owner consumer; actual-binary TERM01 pending.

- R3: VT screen, output replay and resize are native and bounded.
  - Source: donor terminal-pane/session-frame and schema U82/U83/U85.
  - Acceptance: a vetted pinned native VT emulator projects cells, styles and cursor
    into Ratatui; raw PTY escapes are never written directly to the host terminal.
    Match observable donor screen/cursor/input semantics, snapshot then gap-free
    output-cursor attach/replay and ready-before-input ordering. Resize updates the
    actual PTY rows/columns and emulator under the same terminal identity, including
    the session/terminal split and horizontal/vertical tab geometry.
  - Acceptance: bounds cover concurrent PTYs, input/output queues, output ring,
    scrollback, screen dimensions and escape/parser buffers; truncation/gap recovery
    is explicit, not silent invented output. Hidden panes do not stop output draining.
    OSC/clipboard/title/device sequences cannot bypass host-terminal/control/secret
    protections; supported replies route only to the owned PTY. Terminal ANSI16 and
    default fg/bg follow semantic theme roles and update on theme change, separately
    from VIS14/VIS35 code grammar colors.
  - Primary evidence: TERM01 real PTY generates deterministic Unicode/VT/control
    output, delayed attach, bounded floods, input during attach and resize; assert
    screen/cursor/ordered bytes and actual child size, then resource/cleanup facts.
    T44 owns full paired pane/list geometry, RGB/attributes and cursor evidence.
  - Status: in_progress
  - Evidence: evidence/T56/owner.md and frontend.md — real VT Unicode/styles, byte cursor/replay/gap and resize/floods; Ratatui atomic screen/theme/geometry consumer checked, full actual-binary attachment pending.

- R4: Lifecycle remains safe across view changes, shutdown and crash/restart.
  - Source: GOAL A02/A10/no unknown-effect replay and explicit no-daemon boundary.
  - Acceptance: hide/show, composer close, child/parent/session navigation and panel
    selection are not cancellation. Explicit owned remove/exit and clean `oc`
    shutdown close descriptors, terminate remaining owned process groups within
    bounds and wait/reap all owned workers/processes; failures remain non-success.
  - Acceptance: crash recovery verifies recorded process identity before any orphan
    quarantine; stale/unverified PIDs are never killed blindly. Restart reconciles
    retained lifecycle/selection as terminated/interrupted/unknown where appropriate,
    never fabricates running status, recreates an old shell or replays commands.
    Use existing storage for minimal identity/outcome/selection metadata only as
    needed; no second transcript/archive, server handoff or terminal daemon.
  - Primary evidence: TERM01 actual-binary shutdown/crash/restart with two terminals,
    hidden-pane output and stale/session-target controls, plus independent process/
    descriptor checks and A10 measurements. Runtime difference from donor daemon
    persistence is disclosed; it cannot waive pane/list visual qualification.
  - Status: in_progress
  - Evidence: evidence/T56/owner.md and application.md — joined native remove/shutdown, persistent identity/selection, stale-identity refusal and original PTY across Location/reload; crash and full actual-binary qualification pending.

### Constraints and non-goals

- Rust 2024, Linux non-root, four existing crates; `oc-core` has typed ports/DTOs,
  no UI/PTY OS dependency. Preserve existing error/validation/generation model.
- No provider/runner env or credential inheritance/dumps, trust weakening, synthetic
  terminal output, hidden startup spawn, new model-visible terminal tool, external
  scheduler, daemon/serve/attach/HTTP/WebSocket parity, Node/Bun/JS/WASM plugin host.
- Donor uses an opencode-pty daemon and can hand off across client/server lifetimes;
  native PTYs are application-owned. This declared runtime boundary does not turn
  Terminals into a stub or permit removing approved interactive/visual behavior.
- TERM01 has one owner T56. T45 retains child lifecycle, T50 retains shell jobs;
  T44 retains VIS39 presentation and VIS14/VIS35 grammar highlighting. No whole-task
  completion cycle or duplicate security/visual matrix. Existing gates/PASS unchanged.

## Change Envelope

- Add a narrow PTY owner under `crates/oc-adapters/src/`, reuse existing application,
  session/Location/storage/shutdown and shell/env admission seams; add minimal typed
  `oc-core` queries/commands/events and `oc`/`oc-tui` pane/composer consumers.
- Native PTY/VT dependencies may be selected after a compile/license/provenance spike
  and pinned in Cargo.lock; no dependency or code is added by this plan-only patch.
- Substantial new unit tests live separately from production; actual-binary fixtures
  use existing PTY/fake-provider harnesses. No general terminal-service framework,
  store or new crate just for layout. No historical evidence/baseline rewrites.

## Current Checkpoint and State

- Execution authorized 2026-10-06: owner requests the next chronological logical T*
  completed fully, commit/push each checked slice. HEAD `ea6b0b2cc`, tracked baseline
  clean, T53 done, ready T56 precedes T57; `progress.py start T56` succeeded.
- Native bounded PTY/VT owner checked/pushed `88e6631d0`; application source/epoch
  commands, independent lifecycle/capability and migration12 pushed `5b165af61`.
  Frontend composer/right pane, raw/leader/focus, theme cells/atomic attachment and
  retained-child source creation checked: adapters TERM01 8/0, TUI 444/0, binary
  unit 91/0 and strict workspace all-target Clippy green. Partial receipts:
  evidence/T56/owner.md, application.md and frontend.md. All R1–R4 in_progress.
- Next: full rebuilt debug/release actual-binary TERM01 and independent process/
  descriptor/resource facts, including child-close/target and crash/restart; resolve
  termination signal/proc-enumeration failure handling. Full actual-binary remains
  NOT_RUN; no DONE claim.
- Completion remains all R1–R4 plus current TERM01 debug/release actual-binary and
  impacted/final gates. T44 stays PAUSED; paired VIS39 is separate, not waived/PASS.
- Reference inventory: U78–U88 in `tui-recovery/SOURCES.json`; canonical full segment
  and qualification order in T44 amendment and roadmap/M8.md. Consume qualified
  existing owner slices without requiring completion of all T45/T50/T44.

## Material Decision and Completion

- 2026-10-01: owner explicitly includes Terminals. Narrowly supersede the old T50
  arbitrary-terminal-manager exclusion only for user-controlled session PTYs T56;
  shell-tool semantics and credential/permission boundaries are unchanged.
- Final implementation status: pending. Plan checks/commit/push are not TERM01,
  T44/VIS39 visual PASS or product READY.
