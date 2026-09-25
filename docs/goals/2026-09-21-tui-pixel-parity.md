# Goal: TUI pixel parity with opencode v2.0.12

Status: active
Source: user instructions 2026-09-21 and reviewed recovery amendment 2026-09-22, reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (commit `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-24

## Objective

The `oc` TUI renders the upstream opencode v2.0.12 interface: same layout regions, same theme colors, same component inventory (message stream, tool cards, dialogs, command palette, session list, status/footer), same keybindings and visible strings, verified against the running pinned original using identical fixtures, state and terminal profile. Rust's own goldens are regression tests, not proof of external pixel parity.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

The owner's reviewed [T44 amendment](../../tui-recovery/T44_CONTRACT_AMENDMENT.md)
supersedes the weaker self-authored interpretation below. Execute V00–V09 from
[IMPLEMENTATION_GUIDE](../../tui-recovery/IMPLEMENTATION_GUIDE.md), all mandatory
VIS01–VIS28 in [ACCEPTANCE.json](../../tui-recovery/ACCEPTANCE.json), and
[SAFETY_REGRESSIONS](../../tui-recovery/SAFETY_REGRESSIONS.md). These are specifications,
not executed results or a second task engine. `progress.py` remains the task-state owner.

## Frozen Contract

### Required Outcomes

- R1: Reconnaissance artifacts exist and are authoritative.
  - Source: user instruction (recon by part of @general).
  - Acceptance: `evidence/tui/upstream-inventory.md` documents the upstream v2.0.12 TUI: file/component inventory (`packages/tui/src/**`, `packages/theme/src/tui/**`), exact theme colors, layout regions, keymap, and visible strings, each with source path + quoted snippet; `evidence/tui/current-gaps.md` maps our `crates/oc-tui` against it with concrete gaps.
  - Primary evidence: the two files + a reviewer can trace every claim to an upstream path.
  - Status: source inventory verified; executable capture lock pending
  - Evidence: `evidence/tui/upstream-inventory.md` (147 lines, `path:line`-cited: default theme `opencode` dark, 74 token slots + hue scales, ~45 components, layout regions, keymap source, visible strings) and `evidence/tui/current-gaps.md` (171 lines, gap table + test-infrastructure analysis).

- R2: Theme parity — the palette (colors, backgrounds, borders, syntax accents) matches upstream v2.0.12 values, including per-element roles (text, muted, primary, error/warning/success, panel, border, diff add/remove, user/assistant roles).
  - Acceptance: a test asserts our resolved palette equals the upstream values extracted in R1; TUI renders with those colors under a truecolor terminal.
  - Primary evidence: unit test over the palette + PTY snapshot showing colored regions.
  - Status: palette unit tests verified; final rendered colors unverified pending paired frames
  - Evidence: iteration 1 commit `b75e063`; `crates/oc-tui/assets/upstream/v2/opencode.json` (byte-faithful, SHA-256 recorded in `PROVENANCE.md`), `theme.rs::palette_matches_vendored_asset` walks the merged JSON generically (74 slots / 99 hue slots / 173 palette entries per mode), `rendered_frame_carries_theme_styles` asserts colors on a TestBackend frame.

- R3: Layout parity — screen regions (header/logo area, message stream, input editor, status/footer bar, side panels/dialogs) occupy the same positions and respond to resize like upstream.
  - Acceptance: paired styled-cell and PNG captures at 80x24, 120x40, 160x48 and established screenshot grid, plus edge widths 43/44/119/120/121; full-height viewport, sidebar, conditional devtools and real dialogs match the executable reference.
  - Primary evidence: identical-profile original/Rust PTY captures, comparator reports, independent review and resize behavior tests.
  - Status: implemented-partial/unverified
  - Evidence: iteration 2; `crates/oc-tui/src/layout.rs` (upstream geometry constants with source citations) + `shell.rs` golden frames at 80x24 and 120x40, breakpoint resize test, sticky-bottom test; workspace 381 passed / 0 failed / 4 ignored.

- R4: Message rendering parity — user/assistant messages, markdown (headings, lists, code blocks, inline code), reasoning/thinking blocks, tool call cards (command, patch/diff, search, read), errors, and pending/running/completed states match upstream presentation and visible strings.
  - Acceptance: paired captures for scripted transcripts including Markdown tables; live and durable replay restore the same semantic parts/cards/metadata; no raw escape noise; content wraps correctly.
  - Primary evidence: original/Rust paired styled cells and PNG plus real protocol/operation/restart assertions.
  - Status: implemented-partial/unverified
  - Evidence: iterations 3a+3b (committed): `crates/oc-tui/src/messages.rs` renders user blocks with `┃`/raised background/chips, assistant markdown (paddingLeft 3, headings/lists/code fences with syntax colors/blockquotes), collapsed reasoning (`Thinking` → `Thought: … · duration`), and the `agent · model · dur · tok/s · interrupted` footer; additive DTOs `ReasoningDelta`/`TurnUsage`/`duration_ms` wired through the provider stream (2 adapter end-to-end tests). Tool cards: inline rows (read/glob/grep/webfetch/skill/generic with upstream labels and spinner), shell `$ cmd` with stdout/stderr/exit/truncation, apply_patch `# Created`/`← Patched`/`# Deleted` with diff hunks using `diff.text.*` roles, subagent card parsed from the real `<subagent …>` wrapper, pending/running/completed/error/cancelled states; additive `ToolCallStarted/Finished` events emitted after durable writes. Known R4 residual: committed history rows carry no tool cards after a page reload (live turns only).

- R5: Interaction parity — keybindings, command palette, dialogs (session list, model, agent, help, error details), input editor behavior (multi-line, paste, history), inline `/` autocomplete and `@` mention overlays (owner amendment 2026-09-24), mouse text selection/copy, running-agent indicator in the lower-left prompt footer, and status hints match upstream.
  - Acceptance: keymap table test (key → action) mirroring upstream defaults + PTY tests exercising each dialog; paired original/native frames for VIS25/VIS26 trigger, filtered query and after-Tab states; VIS27 verifies upstream mouse selection/copy modes and clipboard feedback; VIS28 verifies the running indicator's animation and state transitions.
  - Primary evidence: keymap test + dialog snapshots + paired running-indicator frame sequence (VIS28).
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
- C2: No hardcoded screenshot text/model labels/Free/Context in production; chrome comes from real DTOs, with honest unknown metadata. No secrets in logs/snapshots.
- C3: Do not weaken existing audited contracts (PTY/terminal restore, DCP panel, permissions, session persistence, accessibility of terminal state).
- C4: `scripts/progress.py` remains the only status source; one active task.
- C5: Application owns acceptance/history/permissions/durability. No capture-only substitution of tool/runtime outcomes or hidden required MCP failures. Missing backend actions require explicit capability mapping/owner scope decision, or a blocked full-parity gate; unavailable actions must not appear working.
- C6: Preserve four crates and current T43/T45 subagent scope. T44 verification does not imply product READY; mandatory live gates, FINAL and fresh security/resource qualification remain required.

### Non-goals

- Web/desktop UI, plugin SDK, sound/clipboard OS integration beyond what upstream TUI visibly shows, animations requiring non-terminal capabilities, upstream Go/Bun production runtime. Original Bun/Node/build dependencies are permitted in an isolated reference/test environment.

## Change Envelope

- Target: `crates/oc-tui/src/**`, `crates/oc/src/{tui_cmd.rs,bootstrap.rs}`, theme/config plumbing in `crates/oc-adapters/src/{config.rs,composition.rs}` if needed, `docs/*`, `evidence/tui/*`, tests under `crates/oc/tests/**` and `crates/oc-tui/src/**`.
- Reviewed extension: minimal existing application DTO/runtime/storage projections in `oc-core`/`oc-adapters`, async submission and safe MCP diagnostics, and test-only reference capture tooling. No second transcript, event bus, plugin host or progress engine.
- Expected paths, symbols, and direct consumers: `views.rs`, `app.rs`, `picker.rs`, `history.rs`, `dcp_panel.rs`, `events.rs`, `commands.rs`, `terminal.rs`; new modules for theme, components, snapshots.
- Allowed and forbidden artifacts: source/tests/docs/evidence. Forbidden: editing GOAL.md gates, deleting tests, adding JS runtime, committing secrets.
- User or harness budget: commits+pushes per slice; iterative rounds; no attempt limit.

## Current Checkpoint

- Closes: no visual gate yet.
- Smallest next action: **owner startup failure root cause is proven by the startup trace and the fix is applied** — the failing zsh exported a stale `LUDKA2_API_KEY` (fingerprint `1101b285`) while the working identity (`54f454fc`) is available from the same `secrets.env` the `oc` alias loads; config resolution, provider and discovery URL matched exactly and the server answered 401 for the stale credential. The owner-approved `with-oc2-secrets` wrapper + `alias oc2` in `~/.zshrc` now load that file before `exec`ing the native binary (backup `~/.zshrc.bak-oc2`; `zsh -n` and a stale-key PTY run both pass with `status=200`/Home). Owner activates it with `source ~/.zshrc` or a new terminal. T44 remains paused; on resumption continue S05/S06/S08, S07 resources and V08–V09 VIS01–VIS26. See `evidence/tui/recovery-startup/{owner-catalog-recon,trace-report,resolved-checkpoint}.md`.
- Expected evidence: capture lock, independent original frames, commands/exit codes and raw input/application effects; exact checkpoint after each slice.
- Stop or replan if: reference/profile unavailable → BLOCKED_REFERENCE, never closest-rendering parity. Independent fixes remain executable.

## Current State

- Resolved: source/theme/component groundwork landed through `d232baa`; historical reports remain unchanged.
- Last relevant evidence: V00 real diagnostic paired captures in `evidence/tui/recovery-v00` (all comparisons unequal); V01 raw PTY stalled MCP prompt and manual compression cancellation/retry plus safe diagnostics in `evidence/tui/recovery-v01/report.md`. These do not qualify VIS01–VIS26.
- Blocker: the owner's exact 401-versus-403 catalog result and upstream policy cannot be determined from this process's product environment; independent T44 work remains executable. Exact capture freeze and missing genuine backend capabilities are open. V07b quarantines uncertain in-flight MCP calls only during the owning process; remote side effects across restart still require reconciliation.
- Next: V06a Markdown and V06b public reasoning/tool/patch/replay verified locally with actual application effects and paired original/native captures; all whole-frame comparisons still DIFFERENT. See `evidence/tui/recovery-v06{a,b}/checkpoint.md`; VIS13–VIS17/R4 remain unverified. V07a–c source/MCP negative checks and V07e one raw-PTY terminal-control path have factual checkpoints; remaining V07 safety/resources, V08–V09 and missing backend/service actions remain open. Owner-reported exported credentials refute the prior missing-export root-cause claim. Their newer 401/403 catalog screen and the current process's separate 200 catalog smoke (report/checkpoint under `evidence/tui/recovery-startup/catalog-smoke-*`) distinguish Responses use from catalog authorization, not the owner's remote policy. Positive live visual inspection is not a waiver; native autoaccept remains Unsupported (`evidence/tui/recovery-v02/auto-capability.md`).

## Material Decisions

- 2026-09-22 resume: incorporate other-agent D13–D15/backend delivery per owner instruction. D13 visible per-server MCP degradation supersedes the earlier fatal attach rule (not cancellation/cleanup/caps). D14 absent variant means no overlay and unknown limits remain metadata-unknown with bounded native request policy. Resource-aware permissions only narrow. Keep historical V01/V04 reports as executed; current tests follow the newer contract. T44 is still the sole active journal task; other task scope is not marked done by integration.
- 2026-09-22 (supersedes the 2026-09-21 source-derived-golden interpretation): pixel-perfect requires the running pinned upstream and Rust under identical fixture/state/profile, exact symbols/styles/colors/cursor/geometry and interaction, paired PNG and styled-cell dumps, comparator and independent review. Own TestBackend expected values cannot establish external parity. Bun/Node are allowed only for the isolated reference. Missing runnable reference/profile means BLOCKED_REFERENCE, not verified R3/R4/R5. Preserve failed/ignored/blocked attempts and rerun full qualification on the final code SHA. TUI_IMPLEMENTED_UNVERIFIED and TUI_PARITY_VERIFIED are reporting labels, not runtime enums; neither overrides product READY gates.

## Checkpoint History

- 2026-09-21: contract frozen; recon delegated.
- 2026-09-21: R1 verified (both recon artifacts); iteration 1 (theme foundation) committed `b75e063`; iteration 2 (layout shell + goldens) committed next.
- 2026-09-22: owner-reviewed recovery amendment accepted; R2 rendered colors and R3/R4 reopened as unverified, historical reports not rewritten.
- 2026-09-22: V00 committed/pushed `679e683`; V01 reproduced both submit/compress freezes, added shared nonblocking receipts and owned MCP attach cleanup. Independent parent PTY verification: 5 passed, fmt/diff checks exit 0. See V01 report and checkpoint; no visual gate closed.
- 2026-09-22: V01 delivered `4c1a0bc`; V02 safe durable projections/title/model/usage and live/replay parts implemented. Parent actual binary recovery test passed twice including final agent-switch correction; full implementation workspace 436 passed/4 existing live ignores. Exact limitations/attempts in V02 report, no parity claim.
- 2026-09-22: V02 delivered `a4bb361`; V03 actual measured viewport, sidebar, dynamic prompt and conditional devtools implemented. Review fixed safe startup/query routes, clamped Down scrolling and channel defaults. Parent binary PTY test/fmt/diff exit 0; implementation workspace 443 passed/4 existing live ignores. Paired attempts 16–20 retain actual scroll/cursor/child/vertical evidence; all whole-frame parity remains open.
- 2026-09-22: resumed at `15719e9` (V04 committed during pause). Preserved backend T47/T43/T46/T27 work; reproduced and corrected three explicit-variant fixture expectations, strengthened raw PTY default-versus-named-none wire/durability check. Fresh workspace 488 passed/0 failed/5 existing ignores, fmt/clippy/build/docs/journal/Python gates passed. Independent V04 review recorded further in-scope dialog gaps; no VIS closure.
- 2026-09-22: V04 follow-up atop `931792e`: real separate variant/dialog registry/new session, app-owned scoped selection and headless precedence, retired choice refusal/recovery, pinned fuzzy oracle/cache and truecolor modal blank correction. Parent fresh workspace 498 passed/0 failed/5 existing ignores; PTY 8 passed; paired original/Rust Session/Commands/Models/Variants at 160x48, 80x24, 121x41 retained with comparator exit1 DIFFERENT. Full V04/backend actions and VIS still open; `evidence/tui/recovery-v04/followup-checkpoint.md`.
- 2026-09-22: V04 modal mouse capture/restore, focus ownership and scroll/hover/select routed through real PTY. Review fixed informational-row compression and cross-dialog press/release. Full serial workspace green (5 existing ignores); no new paired parity qualification. See `evidence/tui/recovery-v04/modal-focus-checkpoint.md`; V05 next.
- 2026-09-22: V05 grapheme editor, focus/keymap and actual compact paste-chip with underlying real prompt, pending revision preservation. Independent review fixed five editor and three chip/ownership defects; parent final serial workspace/clippy/build/fmt/docs/Python exit 0. Actual pinned-original/Rust chip frames match visible chip/cursor, whole-frame comparator grid/png FAIL; see `evidence/tui/recovery-v05/checkpoint.md`. Release startup user report is new next action, no private config read.
- 2026-09-22: user-reported release-startup generic screen diagnosed using isolated actual `target/release/oc`, not their profile. Typed allowlisted startup and Location-switch diagnostics replace raw/indistinguishable errors; independent review's leak and saved-selection category findings fixed. Parent targeted tests and release PTY, full serial workspace (five existing ignores), clippy/fmt/docs/journal/diff all exit 0. Real-profile cause unknown; no visual gate closed. See `evidence/tui/recovery-startup/checkpoint.md` and append-only report.
- 2026-09-22: V06a pinned native Markdown parser, exact captured 11-row table regions, bounded indexed viewport for completed/streaming parts. Independent reviews reproduced/corrected footer truncation, wide glyph/control, page continuity/cache churn, nonprogress on huge grapheme, quoted table/long-cell and escaped-pipe continuation. Parent final targeted29 tests, serial workspace/clippy/fmt/build/docs/journal/diff exit 0; actual paired 80/120 whole-frame comparator exits 1 DIFFERENT. No VIS or R4 parity claim; see `evidence/tui/recovery-v06a/checkpoint.md`.
- 2026-09-23: V06b owner result navigation, reasoning expansion, conservative partial-patch cards and real-provider binary restart. Independent review fixed stale cross-session card previews, clipped result rows, indistinguishable controls and invalid UTF-8/overflow offsets. Parent targeted, serial workspace, fmt/clippy/build/docs/Python/journal/diff exit 0 (five existing ignores). Actual pinned 120x40 reasoning/read pairs remain DIFFERENT; VIS15–VIS17 and R4 still open. `evidence/tui/recovery-v06b/checkpoint.md`.
- 2026-09-23: V07a S02 discovered-source admission before reads; external nested definitions and symlink-swap reproduced then fixed via pinned-root opens, with actual binary fake-provider and bare PTY tests. Independent final review 7 adapter + 6 binary + 1 PTY passed; parent final serialized workspace/fmt/clippy/build/docs/journal/diff exit 0 (five existing ignores). Scope and pre-fix failed attempts in `evidence/tui/recovery-v07a/{report,checkpoint}.md`. V07b/c and visual qualification remain open.
- 2026-09-23: V07b in-flight MCP request-ID cancel and owned teardown; unverified outcomes durable `unknown`, local reap, remote retry quarantine across reload/shutdown/Location. Independent reviews identified three bypass classes; fake effects and PTY reproduced them, then targeted tests and parent full serial workspace/fmt/clippy/build/docs/journal/diff passed (five existing ignores). External server effect/process restart unresolved honestly; see `evidence/tui/recovery-v07b/{report,checkpoint}.md`. V07c and visual qualification remain open.
- 2026-09-23: V07c `{file:}` ancestor-swap exposed an outside fixture value to provider config before repair; pinned-root substitutions and strict independent public source opens now refuse it. Actual binary/PTY fake-provider and independent review passed, parent full serial workspace/fmt/clippy/build/docs/journal/diff exit 0 (five existing ignores). Remaining S03–S08 and visual qualification remain open; `evidence/tui/recovery-v07c/{report,checkpoint}.md`.
- 2026-09-23: V07e S04 actual raw PTY model/reasoning/stdio tool sentinel and replay test passed with existing production rendering; parent focused run and implementing agent full workspace/fmt/clippy/build/docs/diff exit 0 (five existing ignores). Qualification limited to exercised surfaces, not VIS or real-profile startup. Owner requested pause and then commit; next priority real release failure, not more synthetic startup claims. `evidence/tui/recovery-v07e/{report,checkpoint}.md`.
- 2026-09-23: Actual owner-profile RECON and release PTY differentiated inherited/nonexistent selected `LUDKA2_API_KEY`: Home/exit 0 with nonempty inherited key on default or isolated data, old Configuration failure/exit 1 if variable removed, before discovery. Added typed missing-credential TUI screen without raw config/secret; new release negative screen/exit 1 and positive Home/exit 0, termios restored. Parent targeted/release/full serial workspace/fmt/clippy/build/docs/journal/diff exit 0 (five existing ignores). Whether the owner's external zsh exports that variable remains outside this process, and `secrets.env` was not read. See `evidence/tui/recovery-startup/{credential-report,credential-checkpoint}.md`.
- 2026-09-23: Owner's exported key/URL disproved the previous missing-export diagnosis for their case; do not reassert it. Selected model has no static entry: a catalog rejection/failure or a successful list lacking that ID previously collapsed into generic Configuration load failed. DiscoveryOutcome now retains typed safe failure; TUI differentiates authorization, other HTTP, network, invalid/empty catalog and selected-ID absence, with status checked before error body. Test-first fake HTTP/PTy, parent 3 startup+19 discovery tests, release build, full workspace/fmt/clippy/build/docs/journal/diff exit 0 (five existing ignores). Real-profile startup with the agent's distinct credential reaches Home, but the owner's catalog outcome remains unknown and no pasted credential was used. `evidence/tui/recovery-startup/{discovery-report,discovery-checkpoint}.md`.
- 2026-09-23: Owner reports catalog 401/403 under their zsh while original OpenCode 1 uses Responses. Two read-only `/v1/models` GETs using only this process's inherited product credential returned 200/selected ID present with both native and JS-style User-Agent; fresh release real-product-profile Home/exit 0 (no prompt). Test-first split 401 from 403 through safe application/TUI codes; eight release PTY fixtures, 19 discovery tests and fresh full serialized workspace/fmt/clippy/build/docs/journal/diff all exit 0, five existing ignores. This does not establish the owner's exact status or authorizing key; see `evidence/tui/recovery-startup/catalog-smoke-{report,checkpoint}.md`.
- 2026-09-23: V07 S03 current-HEAD actual binary raw PTY test passed Ctrl/Alt/Shift+Enter/release, modal Esc/Ctrl+C, exact provider request and durable prompt. Existing production key routing needed no change; agent workspace passed with five existing ignores, parent targeted/fmt/diff exited 0. Owner explicitly paused T44 to prioritize their startup RECON and requested checkpoint/commit. `evidence/tui/recovery-v07-s03/{report,checkpoint}.md` and `evidence/tui/recovery-startup/owner-catalog-recon.md` preserve facts and the discriminating plan; no VIS gate or owner-specific root cause claimed.
- 2026-09-23: Owner requested file-based startup diagnostics. Added `oc_adapters::trace`: bounded 256 KiB, 0600, per-launch-truncated trace with 512-char single-line messages, no new dependencies, silent no-op on failure; call sites cover bootstrap/TUI/headless, application spawn, config selectors/roots/sources/selected model/provider/credential source/env refs/DCP/definitions, discovery attempts/status/class, storage and typed categories. Parent review reduced the headless stage to `detail_len` so a quoting serde error cannot reach a second surface. Trace unit tests 6, startup integration 3, serial workspace 0 (five existing ignores), clippy/fmt/diff 0; release rebuilt; real-profile smoke Home/exit 0 with credential absent from the trace and working fingerprint `54f454fc`. See `evidence/tui/recovery-startup/trace-{report,checkpoint}.md`; owner-specific cause still unproven.
- 2026-09-23: Owner's traced run resolved the startup cause: the failing shell exported a stale `LUDKA2_API_KEY` (`fingerprint=1101b285`), while `~/.config/opencode/secrets.env` — the file the `oc` alias loads — holds the working identity (`54f454fc`); config root/sources/model/provider/URL matched the working smoke and the catalog answered 401 for the stale key. Fix is credential sourcing, not a binary defect; durable `oc2` wrapper proposed. Recorded in `evidence/tui/recovery-startup/owner-catalog-recon.md`; T44 remains paused with S05/S06/S08, S07 and VIS01–VIS24 open.

- 2026-09-24: Owner amendment adds mandatory inline `/` autocomplete (VIS25) and `@` file mention (VIS26) to R5; implementation slices, deferred skills/non-primary agent sections (T45) and supported differences are recorded in `tui-recovery/T44_CONTRACT_AMENDMENT.md`; ACCEPTANCE.json extended to VIS01–VIS26.

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
