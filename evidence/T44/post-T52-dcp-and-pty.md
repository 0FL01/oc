# T44 — first continuation after completed M10/T52

## Result

Changed structural plan M10/T52 completed R0–R7 / ARCH01–ARCH05 and was pushed
through `fa312e0268f839bf8b26f54899871210bff37e8d`; `evidence/T52/report.md` is its
finite acceptance record. T44 was explicitly restored and its first unfinished
qualification slice continued, without claiming all remaining feature work done.

Detailed/chat `showCompression=true` real recompression now has three exact full
paired displays:14,400 styled cells, three cursors and1,941,888 PNG pixels, no masks.
Native three commits/restart use11 bounded local requests; held display adds none,
owner state remains unchanged and all six PTYs exit naturally. Source-derived
original comparison and all mapping limitations are in dcp-report20260928-05.md.
Coordinator independently checked the sealed result again in
dcp-pair-check20260928-09.json; the captured debug binary still matches the final
fresh build. No reference backfill, native history injection or live credentials.

## Checks

Final full serial gate exit0:
`/home/opencode/.local/share/opencode/tool-output/tool_0e7cf490d001H2BZ6nWPClhHDJ`.
Workspace1191 passed,0 failed,9 existing ignored; TUI415, MCP application28,
actual PTY35. Fmt, strict workspace/all-target Clippy, debug/release build/help,
Python5+15+14 tests, docs/progress, xterm frontend/geometry and diff checks pass.
Advisory inventory229 files,167466 physical lines,6721469 UTF-8 bytes; no >5000.

Two earlier full-gate attempts failed the existing pending-MCP raw-substring check
(v01 and v05). Isolated v01 passed; instrumentation then reproduced a v05 failure
in a24-run probe: before-edit85.014829ms, at2.002279913s the reconstructed screen
already contained `pending draft edited`, but the accumulated normalized VT stream
did not contain contiguous `edited`. Success prints/temporary instrumentation
were removed. Cursor-addressed diff paints can interleave other rows between the
letters; concatenating stripped VT bytes is not the terminal's visible state.

The fixture now uses its existing reconstructed-screen condition under the same
`start + IO_TIMEOUT` two-second deadline. Parser body unchanged, extracted only
to a private test-fixture method for the deterministic fragmented-VT regression.
Input/duplicate Enter/120x40 resize, row40 raw-render proof, Unicode/chip input,
owner/request-count/cancel/reap/cleanup assertions and all timeouts remain intact.
New regression passes;24/24 subsequent actual pending-UI runs pass, followed by
the complete green workspace gate. This corrects observation, not product timing.

## Risks

This is one bounded VIS38 control checkpoint, not full VIS38/V09 or T44 acceptance.
T52's immutable pre/post structural inventories remain accurate to their commits;
the two new T44 regressions are subsequent behavior/verification work, not retroactive
changes to those927 mappings. `.opencode/` untouched; no paid/live effects.

## Next

Continue the remaining mandatory VIS38 controls/negative/running/lifecycle gates
from the current contract; do not promote this three-profile result to full parity.
