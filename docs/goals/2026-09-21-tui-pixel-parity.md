# Goal: TUI pixel parity with opencode v2.0.12

Status: active
Source: user instruction 2026-09-21 ("реализовать полный пиксель перфект TUI для opencode rust; полностью скопировать интерфейс с оригинального opencode 2"), reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-21

## Objective

The `oc` TUI renders the upstream opencode v2.0.12 interface: same layout regions, same theme colors, same component inventory (message stream, tool cards, dialogs, command palette, session list, status/footer), same keybindings and visible strings, verified by golden screen snapshots derived from the upstream sources.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: Reconnaissance artifacts exist and are authoritative.
  - Source: user instruction (recon by part of @general).
  - Acceptance: `evidence/tui/upstream-inventory.md` documents the upstream v2.0.12 TUI: file/component inventory (`packages/tui/src/**`, `packages/theme/src/tui/**`), exact theme colors, layout regions, keymap, and visible strings, each with source path + quoted snippet; `evidence/tui/current-gaps.md` maps our `crates/oc-tui` against it with concrete gaps.
  - Primary evidence: the two files + a reviewer can trace every claim to an upstream path.
  - Status: verified
  - Evidence: `evidence/tui/upstream-inventory.md` (147 lines, `path:line`-cited: default theme `opencode` dark, 74 token slots + hue scales, ~45 components, layout regions, keymap source, visible strings) and `evidence/tui/current-gaps.md` (171 lines, gap table + test-infrastructure analysis).

- R2: Theme parity — the palette (colors, backgrounds, borders, syntax accents) matches upstream v2.0.12 values, including per-element roles (text, muted, primary, error/warning/success, panel, border, diff add/remove, user/assistant roles).
  - Acceptance: a test asserts our resolved palette equals the upstream values extracted in R1; TUI renders with those colors under a truecolor terminal.
  - Primary evidence: unit test over the palette + PTY snapshot showing colored regions.
  - Status: verified
  - Evidence: iteration 1 commit `b75e063`; `crates/oc-tui/assets/upstream/v2/opencode.json` (byte-faithful, SHA-256 recorded in `PROVENANCE.md`), `theme.rs::palette_matches_vendored_asset` walks the merged JSON generically (74 slots / 99 hue slots / 173 palette entries per mode), `rendered_frame_carries_theme_styles` asserts colors on a TestBackend frame.

- R3: Layout parity — screen regions (header/logo area, message stream, input editor, status/footer bar, side panels/dialogs) occupy the same positions and respond to resize like upstream.
  - Acceptance: golden snapshots at fixed terminal sizes (e.g. 80x24, 120x40) match the upstream layout spec from R1.
  - Primary evidence: PTY snapshot tests at two sizes + resize behavior test.
  - Status: verified
  - Evidence: iteration 2; `crates/oc-tui/src/layout.rs` (upstream geometry constants with source citations) + `shell.rs` golden frames at 80x24 and 120x40, breakpoint resize test, sticky-bottom test; workspace 381 passed / 0 failed / 4 ignored.

- R4: Message rendering parity — user/assistant messages, markdown (headings, lists, code blocks, inline code), reasoning/thinking blocks, tool call cards (command, patch/diff, search, read), errors, and pending/running/completed states match upstream presentation and visible strings.
  - Acceptance: golden snapshots for a scripted transcript covering each element; no raw escape noise; content wraps correctly.
  - Primary evidence: snapshot tests over scripted transcripts.
  - Status: in_progress
  - Evidence: iteration 3a (committed): `crates/oc-tui/src/messages.rs` renders user blocks with `┃`/raised background/chips, assistant markdown (paddingLeft 3, headings/lists/code fences with syntax colors/blockquotes), collapsed reasoning (`Thinking` → `Thought: … · duration`), and the `agent · model · dur · tok/s · interrupted` footer; additive DTOs `ReasoningDelta`/`TurnUsage`/`duration_ms` wired through the provider stream (2 adapter end-to-end tests). Remaining for R4: tool cards and diffs (iteration 3b).

- R5: Interaction parity — keybindings, command palette, dialogs (session list, model, agent, help, error details), input editor behavior (multi-line, paste, history), and status hints match upstream.
  - Acceptance: keymap table test (key → action) mirroring upstream defaults + PTY tests exercising each dialog.
  - Primary evidence: keymap test + dialog snapshots.
  - Status: pending
  - Evidence:

- R6: Every slice is committed and pushed; workspace gates stay green (fmt/clippy/tests/build).
  - Source: repo AGENTS.md + user instruction "коммиты пуши делай".
  - Acceptance: `git status` clean, each slice committed and pushed; `cargo test --locked --workspace --no-fail-fast` 0 failures.
  - Primary evidence: git log/status + gate output.
  - Status: pending
  - Evidence:

### Constraints

- C1: Rust 2024, modular monolith, `oc-core` independent of UI, KISS/YAGNI; no Node/Bun/JS host in production; ratatui-based rendering.
- C2: No hardcoded model IDs; no secrets in logs/snapshots.
- C3: Do not weaken existing audited contracts (PTY/terminal restore, DCP panel, permissions, session persistence, accessibility of terminal state).
- C4: `scripts/progress.py` remains the only status source; one active task.

### Non-goals

- Web/desktop UI, plugin SDK, sound/clipboard OS integration beyond what upstream TUI visibly shows, animations requiring non-terminal capabilities, upstream Go/Bun runtime.

## Change Envelope

- Target: `crates/oc-tui/src/**`, `crates/oc/src/{tui_cmd.rs,bootstrap.rs}`, theme/config plumbing in `crates/oc-adapters/src/{config.rs,composition.rs}` if needed, `docs/*`, `evidence/tui/*`, tests under `crates/oc/tests/**` and `crates/oc-tui/src/**`.
- Expected paths, symbols, and direct consumers: `views.rs`, `app.rs`, `picker.rs`, `history.rs`, `dcp_panel.rs`, `events.rs`, `commands.rs`, `terminal.rs`; new modules for theme, components, snapshots.
- Allowed and forbidden artifacts: source/tests/docs/evidence. Forbidden: editing GOAL.md gates, deleting tests, adding JS runtime, committing secrets.
- User or harness budget: commits+pushes per slice; iterative rounds; no attempt limit.

## Current Checkpoint

- Closes: R1
- Smallest next action: run two reconnaissance subagents (upstream inventory + our gap analysis).
- Expected evidence: `evidence/tui/upstream-inventory.md`, `evidence/tui/current-gaps.md`.
- Stop or replan if: upstream TUI source is unavailable or its rendering cannot be expressed in a terminal (then record the deviation and continue with the closest faithful rendering).

## Current State

- Resolved: goal registered; subagent slices 1–4 landed (T43) before this goal.
- Last relevant evidence: upstream tree contains the TUI at `packages/tui/src/**` (452 tui-related files, `.tsx` components + `packages/theme/src/tui/**`); our TUI is 3378 lines across 10 modules.
- Blocker: none.
- Next: R1 recon, then iterative implementation slices (theme → layout → message rendering → interaction).

## Material Decisions

- 2026-09-21: "Pixel-perfect" is interpreted as: exact palette values, region geometry, component inventory, visible strings and keymap extracted from upstream sources, locked by golden snapshots of our renderer (upstream itself cannot be executed in this environment: it needs Bun + its dependency tree). Deviations are recorded explicitly, never silently.

## Checkpoint History

- 2026-09-21: contract frozen; recon delegated.
- 2026-09-21: R1 verified (both recon artifacts); iteration 1 (theme foundation) committed `b75e063`; iteration 2 (layout shell + goldens) committed next.

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
