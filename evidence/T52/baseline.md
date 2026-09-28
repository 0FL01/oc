# T52 R0 baseline — 2026-09-28

## Result
Base before relocation: `ddb6ae2f0cba18c4aa161159899228a7343398eb`.
The supplied planning patch is applied and T52 is active. Rust source is
unchanged. The pre-existing `.opencode/` remains uninspected and unstaged.

## Checks
- Planning patch reverse check, docs/progress and diff structure: PASS.
- New stdlib advisory helper: five unittest scenarios PASS, covering strict
  4999/5000/5001 boundaries, LF/no-LF/CRLF, Unicode bytes, staged/unstaged/new/
  renamed/deleted paths, deduplication, scope/roles, symlinks and visible errors.
- `cargo fmt --all -- --check`: PASS on the actual pinned host.
- `cargo test --workspace --locked`: PASS on the actual host, jobs=3,
  test-threads=1, owned disk TMPDIR; full output:
  `/home/opencode/.local/share/opencode/tool-output/tool_0e74067d50014yeZf6phye6Rm8`.
  An earlier tool invocation ended at its 120s process timeout; no source failure
  was observed. No Cargo/test process remained. The complete rerun used a 900s
  tool allowance, without modifying test deadlines or assertions.
- Exact test/ignored lists captured in `test-inventory-before.json`:
  oc-tui lib 414/0; adapters lib 304/0; oc bin 83/0; runtime integration 91/0;
  pty_t39 integration 35/0. These are identities, not just count baselines.
- Size helper reports all eight expected Rust warnings at the unchanged sizes
  in the T52 spec. It is warning-only and does not write or hash source files.

## Risks
Relocation must preserve complete item bodies, attributes, fixture bytes,
test membership, public paths, and lock/drop/await/effect ordering. No new
feature, dependency, Cargo target or test-only production API is authorized.

## Next
R1 test-owner relocation, checked one owner at a time; R2 state owner plus
input/transcript/tabs/live; R3 runtime facade plus turn/context/MCP; R4 storage/
application tests; R5 same integration targets with shared fixture and thematic
scenarios. The read-only scouts identified the public export, private method,
fixture include and existing approval/support module seams. Update CODE_MAP
and live consumers alongside moves, then exact inventory comparison and R7.
