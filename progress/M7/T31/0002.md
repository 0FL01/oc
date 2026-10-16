## Result

T31 runtime slice implemented and verified over `e4c7036`. CLI and TUI now call
one native application factory: ordered config, exact model catalog, Responses,
Runtime-owned sessions/policy/persistence behind CoreApp commands/events. UI
acceptance follows durable user append; no frontend history writes. Missing
config errors instead of mock fallback. Own default data is XDG_DATA_HOME/oc or
HOME/.local/share/oc. SQLite duplicate is a typed exact primary-key constraint.

## Checks

Initial actual-binary AUD01 failed with no endpoint request (exit 101). Final
AUD01 passed (1/1), PTY suite including AUD02/STORE01 and real-binary UI05 passed
(13/13). A CLI→PTY→CLI restart sent stored history in actual HTTP requests.
Rejected busy input was not persisted. Visible partial text precedes cancellation.
fmt, workspace clippy -D warnings, workspace tests, locked build, --help and
diff whitespace check exited 0. Three pre-existing external-only harnesses are
ignored, not PASS. Exact commands/results: evidence/T31/checks.md.

## Risks

This closes wiring F01, not later protocol/durability/tool findings. Silent-peer
cancellation, typed Responses continuation and memory bounds still need T34/T40.
Config trust/definitions/native mappings need T35; T32/T33 P0 remain. Tests used
only temporary roots and fake endpoints. No live probe. Not READY and not
BUILD_READY_LIVE_BLOCKED. T31 smoke must rerun after T34 protocol changes.

## Next

Record implementation commit, add T31 report and finish using existing utility.
Then start T32 and reproduce patch safety/grammar findings on temp fixtures only.
Changed runtime: adapters application/composition/provider/runtime/storage/tools,
core command/events, headless/TUI consumers; binary application/PTy regressions.
Tracker source remains progress/STATE.json; initial checkpoint 0001 is immutable.
HEAD before implementation commit: e4c7036. No registry fragments need re-merge.
