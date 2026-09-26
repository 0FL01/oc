# T44 bounded real-PTY evidence — 2026-09-26

Behavioral checks PASS on both actual executables in final attempts. Exact
unmasked styled-grid/cursor and PNG parity remains DIFFERENT; runner exit 1 is
intentional factual reporting of those differences, not qualification success.

## Commands and artifacts

```sh
cargo build --locked
node --check scripts/tui_capture/capture.mjs
node --check scripts/tui_capture/bounded.mjs
python3 -m py_compile scripts/tui_capture/bridge.py
node scripts/tui_capture/check_frontend.mjs
node scripts/tui_capture/check_capture_geometry.mjs
git diff --check

node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --geometry true --sample short --sidebar hide --columns 120 --rows 40 --bounded-mode variants --output /home/opencode/ai/oc/evidence/tui/recovery-v00/bounded-variants-04
node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --geometry true --sample short --sidebar hide --columns 120 --rows 40 --bounded-mode shell --output /home/opencode/ai/oc/evidence/tui/recovery-v00/bounded-shell-06
```

Build/syntax/frontend/geometry/diff checks exit 0. Build finished before all
captures; Cargo was released then. Native source is HEAD
`83af88057f837b3d6fd84eb2f6d7f8425fca563b` plus existing uncommitted owner Rust
changes. No Rust file was written by this work. Each capture lock records actual
binary SHA, source manifest and dirty diff. The runner's existing-binary policy
still reports build association as not attested: the successful build above was
external to the runner, not `--build-oc true`.

Every attempt has `commands.json`, `capture.lock.json`, per-origin bridge specs,
`bounded-checks.json`, `protocol.json`, exact PTY `inputs.json`, raw VT and full
frame `.vt`, `.cells.json`, `.txt`, `.render.json`, `.png`. Each paired scenario
has `.grid-diff.json` and `.png-diff.json`. No masks or synthetic outcomes were
introduced.

## Final counts and observations

| Attempt | Frames per side | Actual requests per side | Behavioral status | Exact comparisons |
| --- | ---: | --- | --- | --- |
| `bounded-variants-04` | 9 | 4 transcript + 1 title, all completed/valid | PASS both | 9 grid + 9 PNG DIFFERENT |
| `bounded-shell-06` | 6 | 3 transcript + 1 title, all completed/valid | PASS both | 6 grid + 6 PNG DIFFERENT |

Both modes begin in Build, select a real lowercase dedicated profile through
`/agents`, and capture `Fixture-Reader` / `Fixture-Shell` prompt metadata without
the successful `agent: …` toast. Actual profile instructions are checked in
each subsequent non-title provider request.

Variants use only the declared fixture `fast=high`, `none=low` catalog. Actual
wire inspection verifies `reasoning.effort` absent, high, low, absent over the
initial submission and three Ctrl+T cycles. Historical variant-switch messages
remain real transcript content; the cycle predicate inspects the current prompt
metadata row rather than searching old prose.

Shell returns two real function calls and accepts their actual outputs before
emitting the final answer. Exactly one short line and all 40 numbered long-output
lines return through tool results. Dedicated fixture-only permissions permit
only the two exact printf command resources. Neither command writes a file.
The normal Reader fixture is unchanged. Schema-driven selection records a real
compatibility gap: original v2.0.12 advertises **shell**, not bash; native
advertises **bash**, with argv rather than command. The native function call is
actual bash; claiming an original bash function call would be false. Original
executes quoted `printf 'SHELL-SHORT\n'` and
`printf 'SHELL-LINE-%02d\n' 1 … 40`; native executes equivalent direct printf
argv with the literal backslash-n format. Exact arguments are in protocol logs.

Shell hover changes the actual styled cells, then click expands the real card.
Both PTYs resize to 120×80 for expanded full frames containing lines 01–40 and
the final answer. Repeat click recollapses; both PTYs restore 120×40. Provider
transcript count remains three throughout these interactions.

The real pinned original **does display** `Command exited with code 0.` after
successful Shell output. Native does not. Original collapsed long output omits
36 earlier lines; native omits 34, reflecting the different content/budget.
Native command display also lacks original shell quotes. The completed short
card has a single blank border row between command and output in both frames.
Completed Shell comparison: 816 differing cells out of 4800; cursor equal.
Whole-frame palette, geometry, command quoting, success text, collapse budget,
home chrome, real timing and transcript layout differences remain in evidence.

## Preserved development attempts

All `bounded-variants-01..03` and `bounded-shell-01..05` remain intact.

* First variants attempt incorrectly searched old variant prose when testing
  default restoration; row-local metadata predicate fixed it. Attempt 02 passed
  behavior but reselected the already-default profile.
* First Shell attempts exposed submission timing, native argv-only schema,
  original tool's real `shell` action/name, expanded-output viewport bounds,
  and differing collapse budgets. Substantive fixes used actual schemas,
  dedicated permissions, stable draft submission, resized full frames and
  observed collapse markers. Shell 04 passed behavior but reselected default.
* Variants 03 / Shell 05 start-from-Build attempts failed native preflight:
  native inline custom catalog requires an explicit Build entry. The isolated
  fixture now declares that initial profile; final captures exercise an actual
  selection change successfully.

## Final owner-contract qualification

The owner requirement in task T44 explicitly removes native **synthetic**
`Command exited with code 0.` text and forbids a successful empty-output
placeholder/separator. This takes precedence over the observed original suffix.
An experimental correction added that suffix in `bounded-shell-07..08`; those
captures remain historical, **not** evidence of the delivered owner behavior.
The experiment was removed. Recorded stdout is still preserved verbatim, including
that prose when actually present in stdout; exit zero remains metadata.

Final source-built captures use the above commands with `--build-oc true` and
outputs `bounded-shell-09` and `bounded-variants-06`. Both sides pass their
behavioral checks, with three transcript + one title requests for Shell and four
transcript + one title requests for variants, no invalid requests or extra
provider activity on hover/expand/recollapse. Original/native long-output markers
are 36/34 omitted lines respectively because of the explicit success-text
difference. All 30 unmasked grid/PNG comparisons remain DIFFERENT and both runners
return 1; no VIS/V09 PASS. These final capture locks attest the native source build.

Final serial workspace gate PASS after restoring the owner behavior: fmt; locked
workspace tests (zero failures); all-target Clippy `-D warnings`; locked build;
capture syntax, xterm frontend, docs/progress and diff checks. Output:
`/home/opencode/.local/share/opencode/tool-output/tool_0dd8e54820010JHYPv5cQJpKAw`.
The prior gate `tool_0dd81d5f5001YnvLEfY9y7dJ23` predates the restoration and does
not qualify the final Shell rendering. No authoring-agent configuration changes.
