# T44 V04 interaction and scoped-selection continuation

## Result

Base `931792ef8009ae6ad024cf09c780db029b3c8ef2`; active T44. Original
reference remains pinned at commit `2670273ff17da96f85c5826ced57aa1b368754fa`.
Implemented real separate Select variant, Default versus declared `none`, one
command registry with working `/new`, `/clear`, `/continue`, `/variants`,
`/thinking`, `/effort` and Ctrl+X n; unsupported commands are not advertised.
Application owns persisted session+agent model drafts, per-model variant choice,
headless selection precedence, refusal of retired choices before side effects,
and same-provider immutable public-history projection after switching models.
Independent review exposed retired model/variant startup failure and headless
selection overridden by earlier drafts; both corrected and covered by real
wire/SQLite/PTY tests. Pinned fuzzy ranking has a source-derived external
oracle, cached 10k/8 MiB catalog query and bounded debug/release CPU checks.
No production JS host, second history owner or permission broadening.

## Checks

- Parent `cargo test --locked -p oc-tui --lib`: exit 0, 112 passed.
  `cargo test --locked -p oc --test pty_t39 -- --nocapture`: exit 0, eight
  actual PTY cases including explicit named/default variant, scoped A→B→A,
  new-session aliases, retired-choice recovery, headless precedence/restart.
- Final parent `cargo fmt --all -- --check && cargo test --locked --workspace
  --no-fail-fast --quiet && cargo clippy --locked --workspace --all-targets --
  -D warnings && cargo build --locked && target/debug/oc --help && python3
  scripts/check_docs.py && python3 scripts/progress.py check && python3 -m
  unittest discover -s scripts -p 'test_*.py' && node
  scripts/tui_capture/check_frontend.mjs && node --check
  scripts/tui_capture/capture.mjs && node --check
  scripts/tui_capture/fuzzy_oracle.mjs && git diff --check`: exit 0 at every
  step; workspace 498 passed, 0 failed, 5 preexisting ignored; Python 29
  passed; docs 48 tasks/125 specifications; journal structure check only.
  Prior failures, corrections and the optimized CPU gate are recorded, without
  rewriting history, in `followup-interactions.md`.
- Painted blank cells initially reset their foreground; actual paired frames
  exposed xterm default `#eeeeee` versus upstream explicit `#ffffff`.
  `Color::White` gave ANSI `#eeeeec` in the shared frontend. Replaced it with
  truecolor RGB(255,255,255); parent tests and real captures confirm white
  blank foreground without suppressing any differences. Earlier attempts
  `followup-postreview-160x48` and `followup-finalpaint-160x48` remain intact.
- Fresh final-code paired captures `followup-truecolor-{160x48,80x24,121x41}`:
  each capture command in `commands.json`, exit **1**; original and Rust
  provider contracts executed, all four Session/Commands/Models/Variants frames
  CAPTURED with normal executables, raw PTY, PNG, styled-cell dumps, input bytes,
  fixture/profile/source manifests. All eight grid/PNG comparator runs per size
  report DIFFERENT, not invalid capture or accepted VIS. Independent visual
  inspection of 160×48 variant PNGs shows matching modal geometry/content/cursor,
  but differing session metadata and terminal frame. Source manifest seals
  tracked and new Rust/tool inputs without reading `.opencode/` or owner ZIP.

## Risks

R3/R4/R5 and VIS01–VIS24 remain unverified; component tests and positive
modal-specific visual observations cannot replace an exact whole-frame
comparison. Original Commands inventory includes service/integration/sharing
and missing native owner actions. Those are recorded in `capabilities.md`, not
drawn as working controls. Missing release/status/favorites/recents metadata,
modal mouse/focus details and tab/project/session actions are still V04 scope or
explicit owner decisions; full behavioral parity is not claimed. Fixture path,
duration and token rates are dynamic across captures; reasoning requested
6800ms state still unavailable. V05–V09 and independent final qualification
remain. D13 degraded MCP attach with visible warnings, D14 no implicit variant,
D15 resource narrowing preserved. No product READY.

## Next

Continue smallest V04 dialog gap with actual owner effects and paired evidence:
modal focus/mouse and admitted session/project capabilities; do not mimic
unsupported service controls. Then V05 Unicode editor, V06 Markdown/cards,
V07 safety and V08–V09 full paired acceptance/qualification, checkpoint each
verified slice. Preserve all failed and ignored attempts.
