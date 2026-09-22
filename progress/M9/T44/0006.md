# T44 resumed V04 / backend integration

## Result

Resumed from actual HEAD `15719e976030a8fdaadd843ac13556de237f559d`, not the
pre-pause `1105f67`. V04 and D13 MCP degradation were already committed by other
work. Preserved the complete uncommitted T47/T43/T46/T27 backend delivery and its
historical reports. Owner's resume instruction accepts integration of that work.
Untracked `.opencode/` and owner ZIP were neither read nor staged.

Reproduced exactly three T44 failures: fixture selected `None` but expected `low`.
The geometry fixture now explicitly selects `low`, preserving every expected
geometry row. A separate test asserts no label/overlay for `None`. The actual PTY
fixture now makes named `none` observably nonempty, executes it (wire effort low),
then executes Default (no wire effort, durable selection JSON null). This removes
the old false positive where named `none:{}` was indistinguishable from Default.
No production fallback/permission/discovery behavior was reverted.

Read-only independent V04 review completed after the paused review was cancelled.
Confirmed actual medium modal coordinates/cursor and real model application path;
found remaining blank-cell styles, separate variant-dialog flow, model grouping,
fuzzy search, registry aliases/action availability, modal focus/mouse and paired
size coverage gaps. These remain work, not optional omissions. Unsupported backend
capabilities remain mapped in capabilities.md, not fake working actions.

## Checks

- Before correction: `cargo test --locked -p oc-tui shell::tests -- --nocapture`,
  exit101, 11 passed/3 failed, exactly the stale implicit-low expectations.
- After correction: same shell suite exit0, 15 passed; actual PTY
  `cargo test --locked -p oc --test pty_t39 v04_raw_dialogs -- --nocapture`,
  exit0, 1 passed. No new ignores.
- `cargo test --locked --workspace --no-fail-fast --quiet`: exit0,
  **488 passed, 0 failed, 5 ignored**. Four explicit external/live-server tests
  plus the internal catalog probe (covered in subprocess by offline harness).
- `cargo fmt --all -- --check`, workspace all-target clippy with `--locked` and
  `-D warnings`, `cargo build --locked`, `target/debug/oc --help`: exit0.
- `python3 scripts/check_docs.py`: exit0, 48 tasks/125 detailed specifications.
  `python3 scripts/progress.py check`: exit0 (structure only).
  `python3 -m unittest discover -s scripts -p 'test_*.py'`: exit0, 29 passed.
  `git diff --check`: exit0.
- Parent reviewed backend model/admission/application/MCP projection and ordered
  permission source plus the additive backend integration reports. Independent
  V04 reviewer ran modal-state and actual PTY tests, both exit0.

## Risks

No final VIS/comparator acceptance; previous paired frames remain DIFFERENT.
Owner reports a live visual inspection looked good; this is attributed user
feedback without captured profile/artifacts, not an automated parity result.
D13 supersedes fatal per-server attach semantics: warnings must remain visible;
cancellation/cleanup/caps remain fatal. D14 None means no variant overlay.
T27's durable campaign enforcement is unfinished engineering, and an isolated
authorized product config remains a live prerequisite; no external request made.
T45/MCP remaining scope, in-flight tools/call cancellation, config admission,
Unicode editor/Markdown and final qualification remain open. No task finish/READY.

## Next

Continue V04 required dialog interactions first (separate variant selection,
registry-driven aliases/availability and genuine new-session action), using raw
PTY/application effects and paired captures. Then V05–V09, one verified slice and
existing journal checkpoint/commit at a time. Retain all earlier failed reports.
