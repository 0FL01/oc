# T44 — state-aware sidebar action in Commands

Pinned v2.0.12 `packages/tui/src/component/session-frame.tsx:182-195` registers the real `session.sidebar.toggle` palette action with title **Show sidebar** when its right pane is absent and **Hide sidebar** when visible. Native previously labeled the genuine toggle action **Toggle sidebar** regardless of state. Native `TuiState::modal_options` now derives the title from its last painted session width/rail, Home/child and responsive sidebar visibility; the command ID, shortcut and working action remain unchanged. TestBackend covers both actual states, no observed geometry, the 120/121 and vertical-rail responsive boundaries, Home/child and real action dispatch. Code/runner commit `93b7840`.

## Independent original/native PTY evidence

The runner's opt-in `--sidebar-palette true` uses actual Ctrl+P, types `sidebar`, captures the selected action, sends Return and verifies the painted sidebar toggles. Both initial states are captured independently at 160×48 with a real fixture-backed Reader/tools turn and one shared terminal profile; the runner records the input/VT/protocol, styled cells, PNG and unmasked comparisons. Commands (fresh attempt paths):

```sh
node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true --sample tools \
  --agent-profile true --sidebar hide --columns 160 --rows 48 --sidebar-palette true \
  --output /home/opencode/ai/oc/evidence/tui/sidebar-palette-hidden-20260924-current
# For the second independent run use --sidebar auto and output sidebar-palette-visible-20260924-current.
```

The earlier immutable `sidebar-palette-hidden-20260924-{01,02,final}/` and `sidebar-palette-visible-20260924-{01,final}/` attempts retain probe development and the **prior** observed static title; the two `-current` attempts capture final code. Original executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` (source `2670273ff17da96f85c5826ced57aa1b368754fa`), native executable SHA-256 `89223e8cfbcdfa343a4dba3888eef068fb2d426b1bd0d26db18abc717dbdbf48`, fixture hash `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`; the hidden and visible terminal profile IDs differ **because the explicit sidebar state differs** (`76c3bf1d865117bd2ec08625b5d737928bf1f9575bbe59ca08570a55aac21993`, `50d156640d3a2bd27b172e65def22101e07bba6485084f80c11f3b8dbb49abea`). Within each pair the two sides share the same profile. Native source lock records HEAD `0572d0f` plus dirty diff manifest `7ca81de9ca3e962734a2cc9193b3ca878580957f07d89d95f64c21dc9180dd31`.

Both original and native report `SIDEBAR_PALETTE_ACTION_EFFECT_CONFIRMED`, `provider_contract=true`; with sidebar hidden, they show and execute **Show sidebar**, and with sidebar visible, **Hide sidebar**. The genuine selected option's row `y=17` matches in both captures. Full search frames remain **DIFFERENT** (167/7680 styled cells each): original also offers working Settings results **Sidebar** / **Layout** at `y=18–19`, which native does not have; rendering fake settings rows is not parity. After activation the full frames differ by one actual elapsed digit. Home/Session still differ in true app version or live timings. Runner exit 1 correctly retains nonmatching full grids/PNGs; do not mark VIS08 or VIS24 PASS.

## Checks / remaining scope

Final serialized `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` passed 0 failures (237 TUI tests; existing opt-in live ignored). Workspace fmt, all-target Clippy `-D warnings`, locked build, Node syntax, docs/progress structural checks and diff check passed. The missing owner-backed settings actions, broader Commands/Models, error/replay/resize pixel-edge, S07 and V08–V09/VIS01–24 remain open; `.opencode/` and live credentials were not read.
