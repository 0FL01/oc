# T44 — paired exploration-group RECON and implementation

## Result

Goal of this slice: reproduce the pinned original's default collapsed exploration
row in a *real tool execution* without hiding uncertain outcomes. On upstream
v2.0.12 (`2670273ff17da96f85c5826ced57aa1b368754fa`), adjacent read/glob/grep
parts form a group (`packages/tui/src/routes/session/grouping/session.ts:67-71`),
whose header and counts are rendered by `routes/session/index.tsx:1865-1929`.
An independent 120x40 original/Rust PTY pair executed the same real local
`read` with a fake Responses peer and showed `→ Explored — 1 read` versus
`Read fixture-note.txt` / `↳ Loaded fixture-note.txt` before the change.

Code commit `45bbfdb` groups adjacent read/glob/grep in both transcript paths,
with counts in first-seen order, `Exploring` while an operation runs, and
`Explored` after all finish. Failed, denied, unknown, cancelled and truncated
outcomes are **not** collapsed: their details remain visible. Existing `/cards`
still serves the recorded operations and full durable results, including after
restart; an upstream-style mouse expansion/toggle of the group is not yet
implemented. No operation outcome or owner-side data was changed.

## Checks

- Test-first: the new full/visible-transcript grouping test failed before the
  change (`Read fixture-note.txt` instead of the upstream group), and the
  mixed completed/running test caught a premature `Explored`. Both passed after.
- `cargo test --locked -p oc-tui`: 171 passed, 0 failed. Final
  `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
  --no-fail-fast --quiet`: passed, 0 failed; existing opt-in live tests ignored.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets
  -- -D warnings`, `cargo build --locked`, `python3 scripts/check_docs.py`,
  `python3 scripts/progress.py check`, `git diff --check`: passed on this code.
- Immutable PTY captures: `recovery-v08-read-group-baseline/` (pre-fix),
  `-after/` (initial indent), `-corrected/` (position correction) and `-final/`
  (last code). Each was made with:

  `node scripts/tui_capture/capture.mjs --reference
  /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true
  --sample tools --sidebar hide --columns 120 --rows 40 --output
  /home/opencode/ai/oc/evidence/tui/<attempt>`

  Reference executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
  final Rust executable SHA-256 `bdafbc844863a682ae0b42bb97c4587bbd1b911193fcf4f9c3bcb31ad7ece702`.
  Final lock records source HEAD `89c6ac5` plus dirty tracked diff SHA
  `f24304385f71d2bb162e42d822977c0d0b08bae35385cb48facb971f5e28008e`,
  fixture SHA `dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`
  and shared profile ID `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
  Both sides reported `EXECUTED`, `provider_contract=true`; no real API key
  or user data was used. The final row's symbols, positions and muted colored
  nonblank characters match the original; space foregrounds differ.

## Risks

All four paired runner exits are **1 (DIFFERENT)**, not a parity PASS. The final
120x40 session frame differs at 4580/4800 styled cells and 10025/647040 PNG
pixels (pre-fix baseline: 4631/4800 and 14961/647040). Even the group row
has styled blank-cell differences. Upstream shows `Build · <model> · <duration>`
while this reference fixture's Rust turn has no selected agent ID; do not
invent a Build ID when the application snapshot says `None`. Animated times
and terminal chrome also remain different. The group is not clickable and
there is no `session.grouping=none` toggle in the native UI. VIS16 and all
other unqualified VIS gates remain OPEN. The full T44 is not done.

## Next

RECON the effective default-agent identity in the owner snapshot and upstream
fixture without hardcoding a profile ID; separate absent selection from the
upstream builtin default. Then qualify group expansion/individual inspection
under real keyboard/mouse input and continue the S05/S06/S08/S07 and VIS01–24
matrix with exact paired styled cells and PNGs. Do not turn a local matching
substring or the green offline suite into a full-parity claim.
